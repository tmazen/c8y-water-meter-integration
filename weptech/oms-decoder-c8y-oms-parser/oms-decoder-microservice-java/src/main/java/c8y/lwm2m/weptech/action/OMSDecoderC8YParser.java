package c8y.lwm2m.weptech.action;

import com.cumulocity.microservice.autoconfigure.MicroserviceApplication;
import org.springframework.boot.SpringApplication;

import org.springframework.web.bind.annotation.*;
import org.springframework.http.MediaType;

import com.cumulocity.microservice.customdecoders.api.exception.DecoderServiceException;
import com.cumulocity.microservice.customdecoders.api.model.DecoderInputData;
import com.cumulocity.microservice.customdecoders.api.model.DecoderResult;
import com.cumulocity.model.idtype.GId;
import org.springframework.beans.factory.annotation.Autowired;
import java.io.IOException;

@MicroserviceApplication
@RestController
@RequestMapping(value = "/decode")
public class OMSDecoderC8YParser {

    @Autowired
    OMSDecoderC8YParserService omsSDecoderService;

    public static void main (String[] args) {
        SpringApplication.run(OMSDecoderC8YParser.class, args);
    }

    @RequestMapping(method = RequestMethod.POST, consumes = MediaType.APPLICATION_JSON_VALUE,
            produces = MediaType.APPLICATION_JSON_VALUE)
    @ResponseBody
    public DecoderResult decodeWithJSONInput(@RequestBody DecoderInputData inputData) throws DecoderServiceException, IOException {
        return omsSDecoderService.decode(inputData.getValue(), GId.asGId(inputData.getSourceDeviceId()), inputData.getArgs());
    }
}
